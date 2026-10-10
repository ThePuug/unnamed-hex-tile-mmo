//! Where an entity is and where it is drawn: `Position`, what physics
//! says, and `VisualPosition`, the interpolation the renderer reads.
//!
//! When `Position` changes, `VisualPosition` starts from wherever the
//! entity currently appears toward the new position, never from a point
//! physics computed, so a direction change bends the drawn path without a
//! jump.

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

/// Authoritative position: the tile the entity stands in and its offset
/// from that tile's centre, in world units. The world position is
/// `map.convert(tile) + offset`. The offset may leave the tile; the caller
/// re-bases onto the tile it `reached` (`Position::rebase`).
///
/// The local player's is predicted by physics and confirmed by the server;
/// a remote entity's comes with its intent; `VisualPosition` interpolates
/// toward either.
#[derive(Clone, Component, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Position {
    /// The hex tile this entity occupies
    pub tile: Qrz,
    /// Offset from the tile's centre, in world units
    pub offset: Vec3,
}

impl Position {
    /// Create a new position at the given tile with zero offset
    pub fn at_tile(tile: Qrz) -> Self {
        Self { tile, offset: Vec3::ZERO }
    }

    /// Create a new position with explicit tile and offset
    pub fn new(tile: Qrz, offset: Vec3) -> Self {
        Self { tile, offset }
    }

    /// Convert to world-space position using the map
    pub fn to_world(&self, map: &crate::resources::map::Map) -> Vec3 {
        use qrz::Convert;
        map.convert(self.tile) + self.offset
    }

    /// The tile the offset has reached: the position's own while the offset
    /// is within it, the neighbour it has crossed into once it has left.
    pub fn reached(&self, map: &crate::resources::map::Map) -> Qrz {
        use qrz::Convert;
        self.tile + map.convert(self.offset)
    }

    /// Moves the position to `tile`, keeping where it stands: the offset
    /// changes by the tile difference, exact, so a crossing far from the
    /// origin never lands the position on a float step, as taking it
    /// through a world vector would.
    pub fn rebase(&mut self, tile: Qrz, map: &crate::resources::map::Map) {
        use qrz::Convert;
        let delta: Vec3 = map.convert(self.tile - tile);
        self.offset += delta;
        self.tile = tile;
    }
}

/// Where an entity is drawn: an interpolation in rendered coordinates,
/// about the render origin and never the world's, from where the entity
/// appeared when its target was set to that target, with the waypoints
/// beyond it of a multi-segment path.
#[derive(Clone, Component, Copy, Debug)]
pub struct VisualPosition {
    /// Rendered position the interpolation started from
    pub from: Vec3,
    /// Rendered position it moves toward
    pub to: Vec3,
    /// 0 at `from`, 1 at `to`
    pub progress: f32,
    /// Seconds the segment takes
    pub duration: f32,
    /// Waypoints after `to`, in order
    path: [Vec3; 4],
    /// How many of `path` are set
    path_len: u8,
}

impl Default for VisualPosition {
    fn default() -> Self {
        Self {
            from: Vec3::ZERO,
            to: Vec3::ZERO,
            progress: 1.0, // Start complete (at destination)
            duration: 0.0,
            path: [Vec3::ZERO; 4],
            path_len: 0,
        }
    }
}

impl VisualPosition {
    /// At `position`, with nothing to interpolate
    pub fn at(position: Vec3) -> Self {
        Self {
            from: position,
            to: position,
            progress: 1.0,
            duration: 0.0,
            path: [Vec3::ZERO; 4],
            path_len: 0,
        }
    }

    /// Starts a new interpolation from where the entity currently appears
    /// toward `target` over `duration` seconds, dropping any path. Starting
    /// from the appearance, not a physics point, is what keeps a direction
    /// change from jumping.
    pub fn interpolate_toward(&mut self, target: Vec3, duration: f32) {
        self.from = self.current();
        self.to = target;
        self.progress = 0.0;
        self.duration = duration.max(0.001); // Avoid division by zero
        self.path_len = 0;
    }

    /// Where the entity appears: `progress` of the way from `from` to `to`.
    /// Computed as `from + (to - from) * progress`, never as the two-product
    /// lerp: far from the origin that form's products round apart and the
    /// sum wanders by a float step as the progress moves, with nothing to
    /// move toward, and the camera following it rounds on its own.
    pub fn current(&self) -> Vec3 {
        let progress = self.progress.clamp(0.0, 1.0);
        if progress >= 1.0 {
            return self.to;
        }
        self.from + (self.to - self.from) * progress
    }

    /// Set up multi-segment interpolation along a path of waypoints.
    /// `waypoints` are rendered positions (up to 5: the first becomes `to`,
    /// the rest go into the path buffer). `total_duration` is split evenly.
    pub fn interpolate_along_path(&mut self, waypoints: &[Vec3], total_duration: f32) {
        if waypoints.is_empty() {
            return;
        }
        let segments = waypoints.len() as f32;
        let seg_duration = (total_duration / segments).max(0.001);

        self.from = self.current();
        self.to = waypoints[0];
        self.progress = 0.0;
        self.duration = seg_duration;

        let extra = &waypoints[1..];
        let count = extra.len().min(4);
        for i in 0..count {
            self.path[i] = extra[i];
        }
        self.path_len = count as u8;
    }

    /// Advance the interpolation by delta time (in seconds).
    /// Chains to the next path segment when the current one completes.
    /// Returns true when ALL segments are complete (progress >= 1.0 and no path remaining).
    pub fn advance(&mut self, delta_seconds: f32) -> bool {
        if self.duration > 0.0 {
            self.progress += delta_seconds / self.duration;
        } else {
            self.progress = 1.0;
        }

        // Chain to next segment if current is done and path has more waypoints
        while self.progress >= 1.0 && self.path_len > 0 {
            let overshoot = (self.progress - 1.0) * self.duration;
            self.from = self.to;
            self.to = self.path[0];
            // Shift path entries down
            for i in 0..3 {
                self.path[i] = self.path[i + 1];
            }
            self.path[3] = Vec3::ZERO;
            self.path_len -= 1;
            // duration stays the same (even split)
            self.progress = if self.duration > 0.0 { overshoot / self.duration } else { 1.0 };
        }

        self.progress >= 1.0 && self.path_len == 0
    }

    /// Check if interpolation is complete
    pub fn is_complete(&self) -> bool {
        self.progress >= 1.0
    }

    /// Moves the whole interpolation by `delta`: where it is, where it is
    /// going, and every waypoint after — the render origin moving under it.
    pub fn shift(&mut self, delta: Vec3) {
        self.from += delta;
        self.to += delta;
        for waypoint in &mut self.path {
            *waypoint += delta;
        }
    }

    /// Snap to a position immediately (no interpolation)
    pub fn snap_to(&mut self, position: Vec3) {
        self.from = position;
        self.to = position;
        self.progress = 1.0;
        self.duration = 0.0;
        self.path_len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A position that has walked into a neighbour re-bases onto it with
    /// the same offset near the origin and millions of tiles out, bit for
    /// bit; through a world vector the far offset would land on a float
    /// step.
    #[test]
    fn a_rebase_keeps_the_place_however_far_out() {
        use qrz::Convert;
        let map = crate::resources::map::Map::new(qrz::Map::new(1.0, 0.8));
        let step: Vec3 = map.convert(Qrz { q: 1, r: 0, z: 1 });
        let walk = |at: Qrz| {
            let mut position = Position::new(at, step * 0.6 + Vec3::new(0.0, 0.05, 0.11));
            let reached = position.reached(&map);
            assert_eq!(reached - at, Qrz { q: 1, r: 0, z: 1 }, "crossed into the neighbour a level up");
            position.rebase(reached, &map);
            position
        };
        let near = walk(Qrz { q: 0, r: 0, z: 0 });
        let far = walk(Qrz { q: -1_600_000, r: 2_400_000, z: 5 });
        assert_eq!(near.offset, far.offset);
        assert!((near.offset - (step * 0.6 + Vec3::new(0.0, 0.05, 0.11) - step)).length() < 1e-6, "{:?}", near.offset);
    }

    /// Standing still far from the origin, the visual holds one value as
    /// its progress moves, and a move between two points never steps back.
    #[test]
    fn the_visual_holds_still_far_from_the_origin() {
        let far = Vec3::new(-87366.0, 405.6, -41009.766);
        let mut visual = VisualPosition::at(far);
        visual.interpolate_toward(far, 0.1);
        for _ in 0..200 {
            visual.advance(0.001);
            assert_eq!(visual.current(), far);
        }
        let mut walk = VisualPosition::at(far);
        let there = far + Vec3::new(0.08, 0.0, -0.05);
        walk.interpolate_toward(there, 0.1);
        let mut last = walk.current();
        for _ in 0..200 {
            walk.advance(0.001);
            let now = walk.current();
            assert!((now - last).dot(there - far) >= 0.0, "stepped back: {last:?} -> {now:?}");
            last = now;
        }
        assert_eq!(walk.current(), there);
    }

    /// An interpolation starts from where the entity appears, so a change
    /// of direction part way along continues from there without a jump.
    #[test]
    fn test_visual_position_direction_change_no_jump() {
        let mut vis = VisualPosition::at(Vec3::ZERO);

        // Start moving right
        vis.interpolate_toward(Vec3::new(10.0, 0.0, 0.0), 1.0);
        assert!(!vis.is_complete());
        assert_eq!(vis.progress, 0.0);
        assert_eq!(vis.from, Vec3::ZERO);
        assert_eq!(vis.to, Vec3::new(10.0, 0.0, 0.0));
        vis.advance(0.5); // Now at (5, 0, 0)

        let pos_before_direction_change = vis.current();
        assert!((pos_before_direction_change.x - 5.0).abs() < 0.01);

        // Change direction: now moving up instead
        vis.interpolate_toward(Vec3::new(5.0, 10.0, 0.0), 1.0);

        // Key assertion: from should be our current visual position, not some physics position
        assert!((vis.from.x - 5.0).abs() < 0.01, "from.x should be ~5.0 (current visual), got {}", vis.from.x);
        assert!((vis.from.y - 0.0).abs() < 0.01, "from.y should be ~0.0 (current visual), got {}", vis.from.y);

        // Immediately after direction change, visual position should not have jumped
        let pos_after_direction_change = vis.current();
        assert!((pos_after_direction_change.x - pos_before_direction_change.x).abs() < 0.01,
            "Visual position should not jump on direction change");
    }

    #[test]
    fn test_visual_position_zero_duration_completes_immediately() {
        let mut vis = VisualPosition::at(Vec3::ZERO);
        vis.interpolate_toward(Vec3::new(10.0, 0.0, 0.0), 0.0);

        // With zero duration, should complete immediately on any advance
        let complete = vis.advance(0.001);
        assert!(complete);
    }

    /// A progress past either end of the segment appears at that end.
    #[test]
    fn test_progress_clamped_in_current() {
        let at = |progress: f32| VisualPosition {
            from: Vec3::ZERO,
            to: Vec3::new(10.0, 0.0, 0.0),
            progress,
            duration: 1.0,
            path: [Vec3::ZERO; 4],
            path_len: 0,
        };
        assert_eq!(at(2.0).current(), Vec3::new(10.0, 0.0, 0.0), "Progress > 1.0 should clamp to target");
        assert_eq!(at(-0.5).current(), Vec3::ZERO, "Negative progress should clamp to from");
    }

    // ===== Multi-Segment Path Tests =====

    #[test]
    fn test_interpolate_along_path_two_segments() {
        let mut vis = VisualPosition::at(Vec3::ZERO);
        vis.interpolate_along_path(
            &[Vec3::new(10.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0)],
            2.0,
        );

        // First segment: duration=1.0, from=ZERO, to=(10,0,0)
        assert_eq!(vis.from, Vec3::ZERO);
        assert_eq!(vis.to, Vec3::new(10.0, 0.0, 0.0));
        assert!(!vis.advance(0.5)); // midway through segment 1
        let c = vis.current();
        assert!((c.x - 5.0).abs() < 0.01);

        assert!(!vis.advance(0.5)); // end of segment 1 → chains to segment 2
        // Now interpolating from (10,0,0) to (20,0,0)
        let c2 = vis.current();
        assert!((c2.x - 10.0).abs() < 0.5, "Should be near start of segment 2, got {}", c2.x);

        assert!(vis.advance(1.0)); // complete segment 2
        let c3 = vis.current();
        assert!((c3.x - 20.0).abs() < 0.01, "Should reach end, got {}", c3.x);
    }

    #[test]
    fn test_path_overshoot_carries() {
        let mut vis = VisualPosition::at(Vec3::ZERO);
        vis.interpolate_along_path(
            &[Vec3::new(10.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0)],
            2.0,
        );
        // Advance 1.5s in one shot: should overshoot seg1 by 0.5s into seg2
        assert!(!vis.advance(1.5));
        let c = vis.current();
        assert!((c.x - 15.0).abs() < 0.5, "Overshoot should carry into segment 2, got {}", c.x);
    }

    #[test]
    fn test_interpolate_toward_clears_path() {
        let mut vis = VisualPosition::at(Vec3::ZERO);
        vis.interpolate_along_path(
            &[Vec3::new(10.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0)],
            2.0,
        );
        vis.interpolate_toward(Vec3::new(5.0, 0.0, 0.0), 1.0);
        // Should complete after single segment
        assert!(vis.advance(1.0));
    }

    #[test]
    fn test_snap_to_clears_path() {
        let mut vis = VisualPosition::at(Vec3::ZERO);
        vis.interpolate_along_path(
            &[Vec3::new(10.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0)],
            2.0,
        );
        vis.snap_to(Vec3::new(50.0, 0.0, 0.0));
        assert!(vis.is_complete());
        assert_eq!(vis.current(), Vec3::new(50.0, 0.0, 0.0));
        // advance should return true immediately (no path)
        assert!(vis.advance(0.1));
    }

    #[test]
    fn test_empty_waypoints_noop() {
        let mut vis = VisualPosition::at(Vec3::new(5.0, 0.0, 0.0));
        vis.interpolate_along_path(&[], 1.0);
        assert_eq!(vis.current(), Vec3::new(5.0, 0.0, 0.0));
        assert!(vis.is_complete());
    }
}
