pub mod ally_target;
pub mod behaviour;
pub mod entity_type;
pub mod gcd;
pub mod heading;
pub mod keybits;
pub mod offset;
pub mod projectile;
pub mod reaction_queue;
pub mod resources;
pub mod spawner;
pub mod target;
pub mod tier_lock;

use bevy::prelude::*;
use qrz::Qrz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Eq, PartialEq, Serialize)]
pub struct Loc(Qrz);

impl Loc {
    pub fn from_qrz(q: i16, r: i16, z: i16) -> Self {
        Loc(Qrz { q, r, z })
    }

    pub fn new(qrz: Qrz) -> Self {
        Loc(qrz)
    }

    /// Check if two locations are adjacent for melee combat with sloping terrain
    ///
    /// Two locations are considered adjacent if:
    /// - They are on the same tile (flat_distance == 0) - for multiple entities on same hex
    /// - OR they are 1 hex apart horizontally (flat_distance == 1) AND the vertical
    ///   difference is at most 1 tile (|z_diff| <= 1)
    ///
    /// This allows melee attacks up/down slopes but prevents attacks
    /// against targets that are too high/low (e.g., 2+ tiles above/below)
    pub fn is_adjacent(&self, other: &Loc) -> bool {
        let flat_dist = self.flat_distance(other);
        let z_diff = (self.z - other.z).abs();

        // Same tile is always adjacent (multiple entities on same hex)
        if flat_dist == 0 {
            return true;
        }

        // Otherwise, must be 1 hex away with at most 1 z-level difference
        flat_dist == 1 && z_diff <= 1
    }

    /// Check if a target location is reachable within a given number of steps
    ///
    /// Uses pathfinding to determine if there's a viable path from `from` to `to`
    /// that is at most `max_steps` long. Accounts for terrain and slopes (±1 z-level).
    ///
    /// # Early Exit Optimization
    /// Returns false immediately if flat_distance exceeds max_steps (no pathfinding needed)
    ///
    /// # Arguments
    /// * `from` - Starting location
    /// * `to` - Target location
    /// * `max_steps` - Maximum number of steps allowed in the path
    /// * `map` - The game map for pathfinding
    ///
    /// # Returns
    /// `true` if a path exists within max_steps, `false` otherwise
    pub fn is_reachable_within(from: Loc, to: Loc, max_steps: u32, map: &crate::common::resources::map::Map) -> bool {
        use pathfinding::prelude::*;

        // Early exit: if flat distance exceeds max_steps, it's definitely unreachable
        let flat_dist = from.flat_distance(&to) as u32;
        if flat_dist > max_steps {
            return false;
        }

        // Use map.find to get actual ground positions (accounting for terrain)
        let Some((start, _)) = map.find(*from, -60) else { return false };
        let Some((dest, _)) = map.find(*to, -60) else { return false };

        // If start and dest are the same, it's reachable
        if start == dest {
            return true;
        }

        // Use BFS (breadth-first search) to find shortest path
        // BFS is better than A* here because we only care about path length, not optimality
        let result = bfs(
            &start,
            |&loc| {
                // Get neighbors that are passable (map.neighbors handles ±1 z-level)
                map.neighbors(loc)
                    .into_iter()
                    .map(|(neighbor_loc, _)| neighbor_loc)
            },
            |&loc| loc == dest
        );

        // Check if path exists and is within max_steps
        match result {
            Some(path) => {
                // path.len() includes the start position, so subtract 1 for actual steps
                let steps = path.len().saturating_sub(1);
                steps <= max_steps as usize
            }
            None => false
        }
    }
}

#[cfg(test)]
mod loc_tests {
    use super::*;

    #[test]
    fn test_is_adjacent_same_level() {
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 1, r: 0, z: 0 });
        assert!(loc1.is_adjacent(&loc2), "Same level, 1 hex apart should be adjacent");
    }

    #[test]
    fn test_is_adjacent_one_level_up() {
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 1, r: 0, z: 1 });
        assert!(loc1.is_adjacent(&loc2), "1 level up, 1 hex apart should be adjacent (slope)");
    }

    #[test]
    fn test_is_adjacent_one_level_down() {
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 1 });
        let loc2 = Loc::new(Qrz { q: 1, r: 0, z: 0 });
        assert!(loc1.is_adjacent(&loc2), "1 level down, 1 hex apart should be adjacent (slope)");
    }

    #[test]
    fn test_not_adjacent_two_levels_up() {
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 1, r: 0, z: 2 });
        assert!(!loc1.is_adjacent(&loc2), "2 levels up should not be adjacent (too steep)");
    }

    #[test]
    fn test_not_adjacent_two_hexes_away() {
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 2, r: 0, z: 0 });
        assert!(!loc1.is_adjacent(&loc2), "2 hexes apart should not be adjacent");
    }

    #[test]
    fn test_adjacent_same_tile() {
        // Same tile is adjacent (for multiple entities on same hex)
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        assert!(loc1.is_adjacent(&loc2), "Same tile should be adjacent (multiple entities on same hex)");
    }

    #[test]
    fn test_adjacent_same_tile_different_z() {
        // Same horizontal position but different z should be adjacent
        let loc1 = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let loc2 = Loc::new(Qrz { q: 0, r: 0, z: 1 });
        assert!(loc1.is_adjacent(&loc2), "Same tile, different z should be adjacent");
    }

    // ===== REACHABILITY TESTS =====

    #[test]
    fn test_is_reachable_within_early_exit() {
        use crate::common::{components::entity_type::*, resources::map::Map};
        use qrz::Map as QrzMap;

        // Create a simple map
        let mut qrz_map: QrzMap<EntityType> = QrzMap::new(1.0, 0.8);
        qrz_map.insert(Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(Default::default()));
        qrz_map.insert(Qrz { q: 5, r: 0, z: -5 }, EntityType::Decorator(Default::default()));
        let map = Map::new(qrz_map);

        let from = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let to = Loc::new(Qrz { q: 5, r: 0, z: -5 });

        // Target is 5 hexes away, max_steps is 3 - should early exit without pathfinding
        assert!(!Loc::is_reachable_within(from, to, 3, &map),
            "Should early exit when flat_distance exceeds max_steps");
    }

    #[test]
    fn test_is_reachable_within_straight_line() {
        use crate::common::{components::entity_type::*, resources::map::Map};
        use qrz::Map as QrzMap;

        // Create a straight line path
        let mut qrz_map: QrzMap<EntityType> = QrzMap::new(1.0, 0.8);
        for i in 0..=3 {
            qrz_map.insert(Qrz { q: i, r: 0, z: 0 }, EntityType::Decorator(Default::default()));
        }
        let map = Map::new(qrz_map);

        let from = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let to = Loc::new(Qrz { q: 3, r: 0, z: 0 });

        assert!(Loc::is_reachable_within(from, to, 3, &map),
            "Should be reachable in exactly 3 steps");
        assert!(Loc::is_reachable_within(from, to, 5, &map),
            "Should be reachable with more than needed steps");
        assert!(!Loc::is_reachable_within(from, to, 2, &map),
            "Should not be reachable with too few steps");
    }

    #[test]
    fn test_is_reachable_within_with_slopes() {
        use crate::common::{components::entity_type::*, resources::map::Map};
        use qrz::Map as QrzMap;

        // Create a path with slopes (±1 z-level)
        let mut qrz_map: QrzMap<EntityType> = QrzMap::new(1.0, 0.8);
        qrz_map.insert(Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(Default::default()));
        qrz_map.insert(Qrz { q: 1, r: 0, z: 1 }, EntityType::Decorator(Default::default())); // Up 1
        qrz_map.insert(Qrz { q: 2, r: 0, z: 1 }, EntityType::Decorator(Default::default())); // Flat
        qrz_map.insert(Qrz { q: 3, r: 0, z: 0 }, EntityType::Decorator(Default::default())); // Down 1
        let map = Map::new(qrz_map);

        let from = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let to = Loc::new(Qrz { q: 3, r: 0, z: 0 });

        assert!(Loc::is_reachable_within(from, to, 3, &map),
            "Should be reachable via slopes in 3 steps");
    }

    #[test]
    fn test_is_reachable_within_same_location() {
        use crate::common::{components::entity_type::*, resources::map::Map};
        use qrz::Map as QrzMap;

        let mut qrz_map: QrzMap<EntityType> = QrzMap::new(1.0, 0.8);
        qrz_map.insert(Qrz { q: 0, r: 0, z: 0 }, EntityType::Decorator(Default::default()));
        let map = Map::new(qrz_map);

        let loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        assert!(Loc::is_reachable_within(loc, loc, 0, &map),
            "Same location should always be reachable");
    }
}

/// Destination for pathfinding - the Qrz location an entity is trying to reach
#[derive(Clone, Component, Copy, Debug, Default, Deref, DerefMut, Deserialize, Eq, PartialEq, Serialize)]
pub struct Dest(pub Qrz);

#[derive(Clone, Component, Copy, Debug, Default)]
pub struct AirTime {
    pub state: Option<i16>,
    pub step: Option<i16>,
}

#[derive(Clone, Component, Copy, Default)]
pub struct Actor;

/// Attributes for actor entities that affect gameplay mechanics
#[derive(Clone, Component, Copy, Debug, Deserialize, Serialize)]
pub struct ActorAttributes {
    // MIGHT ↔ GRACE (Physical Expression)
    // -100 = pure Might, 0 = balanced, +100 = pure Grace
    pub might_grace_axis: i8,
    pub might_grace_spectrum: u8,
    pub might_grace_shift: i8,  // Player's chosen shift from axis (within ±spectrum)

    // VITALITY ↔ FOCUS (Endurance Type)
    // -100 = pure Vitality, 0 = balanced, +100 = pure Focus
    pub vitality_focus_axis: i8,
    pub vitality_focus_spectrum: u8,
    pub vitality_focus_shift: i8,

    // INSTINCT ↔ PRESENCE (Engagement Style)
    // -100 = pure Instinct, 0 = balanced, +100 = pure Presence
    pub instinct_presence_axis: i8,
    pub instinct_presence_spectrum: u8,
    pub instinct_presence_shift: i8,
}

impl ActorAttributes {
    /// Create new ActorAttributes with raw axis, spectrum, and shift values
    pub fn new(
        might_grace_axis: i8,
        might_grace_spectrum: u8,
        might_grace_shift: i8,
        vitality_focus_axis: i8,
        vitality_focus_spectrum: u8,
        vitality_focus_shift: i8,
        instinct_presence_axis: i8,
        instinct_presence_spectrum: u8,
        instinct_presence_shift: i8,
    ) -> Self {
        Self {
            might_grace_axis,
            might_grace_spectrum,
            might_grace_shift,
            vitality_focus_axis,
            vitality_focus_spectrum,
            vitality_focus_shift,
            instinct_presence_axis,
            instinct_presence_spectrum,
            instinct_presence_shift,
        }
    }

    // === Private: Get current position (axis + shift) ===

    fn might_grace(&self) -> i8 {
        (self.might_grace_axis as i16 + self.might_grace_shift as i16).clamp(-100, 100) as i8
    }

    fn vitality_focus(&self) -> i8 {
        (self.vitality_focus_axis as i16 + self.vitality_focus_shift as i16).clamp(-100, 100) as i8
    }

    fn instinct_presence(&self) -> i8 {
        (self.instinct_presence_axis as i16 + self.instinct_presence_shift as i16).clamp(-100, 100) as i8
    }

    // === MIGHT ↔ GRACE ===

    /// Maximum Might reach with full spectrum shift
    pub fn might_reach(&self) -> u8 {
        if self.might_grace_axis <= 0 {
            // On might side or balanced: abs(axis - spectrum * 1.5)
            ((self.might_grace_axis as i16 - (self.might_grace_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On grace side: spectrum * 1.5
            (self.might_grace_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Maximum Grace reach with full spectrum shift
    pub fn grace_reach(&self) -> u8 {
        if self.might_grace_axis >= 0 {
            // On grace side or balanced: abs(axis + spectrum * 1.5)
            ((self.might_grace_axis as i16 + (self.might_grace_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On might side: spectrum * 1.5
            (self.might_grace_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Current available Might
    pub fn might(&self) -> u8 {
        if self.might_grace_axis <= 0 {
            // On might side or balanced: abs(axis + shift * 0.5 - spectrum)
            ((self.might_grace_axis as i16 + (self.might_grace_shift as i16 / 2) - self.might_grace_spectrum as i16).abs()).max(0) as u8
        } else {
            // On grace side: spectrum - shift * 0.5
            (self.might_grace_spectrum as i16 - (self.might_grace_shift as i16 / 2)).max(0) as u8
        }
    }

    /// Current available Grace
    pub fn grace(&self) -> u8 {
        if self.might_grace_axis >= 0 {
            // On grace side or balanced: abs(axis + shift * 0.5 + spectrum)
            ((self.might_grace_axis as i16 + (self.might_grace_shift as i16 / 2) + self.might_grace_spectrum as i16).abs()).max(0) as u8
        } else {
            // On might side: spectrum + shift * 0.5
            (self.might_grace_spectrum as i16 + (self.might_grace_shift as i16 / 2)).max(0) as u8
        }
    }

    // === VITALITY ↔ FOCUS ===

    /// Maximum Vitality reach with full spectrum shift
    pub fn vitality_reach(&self) -> u8 {
        if self.vitality_focus_axis <= 0 {
            // On vitality side or balanced: abs(axis - spectrum * 1.5)
            ((self.vitality_focus_axis as i16 - (self.vitality_focus_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On focus side: spectrum * 1.5
            (self.vitality_focus_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Maximum Focus reach with full spectrum shift
    pub fn focus_reach(&self) -> u8 {
        if self.vitality_focus_axis >= 0 {
            // On focus side or balanced: abs(axis + spectrum * 1.5)
            ((self.vitality_focus_axis as i16 + (self.vitality_focus_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On vitality side: spectrum * 1.5
            (self.vitality_focus_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Current available Vitality
    pub fn vitality(&self) -> u8 {
        if self.vitality_focus_axis <= 0 {
            // On vitality side or balanced: abs(axis + shift * 0.5 - spectrum)
            ((self.vitality_focus_axis as i16 + (self.vitality_focus_shift as i16 / 2) - self.vitality_focus_spectrum as i16).abs()).max(0) as u8
        } else {
            // On focus side: spectrum - shift * 0.5
            (self.vitality_focus_spectrum as i16 - (self.vitality_focus_shift as i16 / 2)).max(0) as u8
        }
    }

    /// Current available Focus
    pub fn focus(&self) -> u8 {
        if self.vitality_focus_axis >= 0 {
            // On focus side or balanced: abs(axis + shift * 0.5 + spectrum)
            ((self.vitality_focus_axis as i16 + (self.vitality_focus_shift as i16 / 2) + self.vitality_focus_spectrum as i16).abs()).max(0) as u8
        } else {
            // On vitality side: spectrum + shift * 0.5
            (self.vitality_focus_spectrum as i16 + (self.vitality_focus_shift as i16 / 2)).max(0) as u8
        }
    }

    // === INSTINCT ↔ PRESENCE ===

    /// Maximum Instinct reach with full spectrum shift
    pub fn instinct_reach(&self) -> u8 {
        if self.instinct_presence_axis <= 0 {
            // On instinct side or balanced: abs(axis - spectrum * 1.5)
            ((self.instinct_presence_axis as i16 - (self.instinct_presence_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On presence side: spectrum * 1.5
            (self.instinct_presence_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Maximum Presence reach with full spectrum shift
    pub fn presence_reach(&self) -> u8 {
        if self.instinct_presence_axis >= 0 {
            // On presence side or balanced: abs(axis + spectrum * 1.5)
            ((self.instinct_presence_axis as i16 + (self.instinct_presence_spectrum as i16 * 3 / 2)).abs()) as u8
        } else {
            // On instinct side: spectrum * 1.5
            (self.instinct_presence_spectrum as i16 * 3 / 2) as u8
        }
    }

    /// Current available Instinct
    pub fn instinct(&self) -> u8 {
        if self.instinct_presence_axis <= 0 {
            // On instinct side or balanced: abs(axis + shift * 0.5 - spectrum)
            ((self.instinct_presence_axis as i16 + (self.instinct_presence_shift as i16 / 2) - self.instinct_presence_spectrum as i16).abs()).max(0) as u8
        } else {
            // On presence side: spectrum - shift * 0.5
            (self.instinct_presence_spectrum as i16 - (self.instinct_presence_shift as i16 / 2)).max(0) as u8
        }
    }

    /// Current available Presence
    pub fn presence(&self) -> u8 {
        if self.instinct_presence_axis >= 0 {
            // On presence side or balanced: abs(axis + shift * 0.5 + spectrum)
            ((self.instinct_presence_axis as i16 + (self.instinct_presence_shift as i16 / 2) + self.instinct_presence_spectrum as i16).abs()).max(0) as u8
        } else {
            // On instinct side: spectrum + shift * 0.5
            (self.instinct_presence_spectrum as i16 + (self.instinct_presence_shift as i16 / 2)).max(0) as u8
        }
    }

    // === DERIVED ATTRIBUTES ===

    /// Movement speed derived from grace (might_grace position)
    /// Higher grace = higher movement speed
    /// Formula from ADR-010: max(75, 100 + (grace / 2))
    ///
    /// Grace = -100 (Might specialist): speed = 75% (clamped, 0.00375)
    /// Grace = 0 (parity): speed = 100% (baseline, 0.005)
    /// Grace = 50: speed = 125% (+25%, 0.00625)
    /// Grace = 100 (Grace specialist): speed = 150% (+50%, 0.0075)
    pub fn movement_speed(&self) -> f32 {
        const BASE_SPEED: f32 = 0.005;  // World units per millisecond (MOVEMENT_SPEED from physics.rs)

        let grace = self.might_grace() as f32;  // -100 to +100
        let speed_percent = (100.0 + (grace / 2.0)).max(75.0);  // 75 to 150
        BASE_SPEED * (speed_percent / 100.0)
    }

    /// Maximum health derived from vitality_reach
    /// Higher vitality reach = higher max health potential
    /// 0 vitality_reach = 100 HP
    /// 100 vitality_reach = 2000 HP
    pub fn max_health(&self) -> f32 {
        let base = 100.0;
        let vitality_reach = self.vitality_reach() as f32;
        base + (vitality_reach * 19.0)
    }
}

impl Default for ActorAttributes {
    fn default() -> Self {
        Self {
            might_grace_axis: 0,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            vitality_focus_axis: 0,
            vitality_focus_spectrum: 0,
            vitality_focus_shift: 0,
            instinct_presence_axis: 0,
            instinct_presence_spectrum: 0,
            instinct_presence_shift: 0,
        }
    }
}

#[derive(Clone, Component, Copy, Default)]
pub struct Physics;

#[derive(Debug, Default, Component)]
pub struct Sun();

#[derive(Debug, Default, Component)]
pub struct Moon();

/// Tracks the last time an auto-attack was performed (ADR-009)
/// Used to enforce 1.5s cooldown between passive auto-attacks
#[derive(Clone, Component, Copy, Debug)]
pub struct LastAutoAttack {
    /// Game time when last auto-attack was performed (server time + offset)
    pub last_attack_time: std::time::Duration,
}

impl Default for LastAutoAttack {
    fn default() -> Self {
        Self {
            last_attack_time: std::time::Duration::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balanced_attributes() {
        // axis=-50, spectrum=25: -50A/25S (on might side)
        // might_reach = abs(-50 - 25*1.5) = abs(-50 - 37) = 87
        // grace_reach = 25 * 1.5 = 37
        let base = ActorAttributes {
            might_grace_axis: -50,
            might_grace_spectrum: 25,
            ..Default::default()
        };

        let attrs = ActorAttributes { might_grace_shift: -25, ..base };
        assert_eq!(attrs.might_reach(), 87);  // abs(-50 - 37)
        assert_eq!(attrs.might(), 87);  // abs(-50 + (-25)/2 - 25) = abs(-87) = 87
        assert_eq!(attrs.grace(), 13);  // 25 + (-25)/2 = 25 + (-12) = 13 (integer division)
        assert_eq!(attrs.grace_reach(), 37);  // 25 * 1.5

        let attrs = ActorAttributes { might_grace_shift: -10, ..base };
        assert_eq!(attrs.might_reach(), 87);
        assert_eq!(attrs.might(), 80);  // abs(-50 + (-10)/2 - 25) = abs(-80) = 80
        assert_eq!(attrs.grace(), 20);  // 25 + (-10)/2 = 25 - 5 = 20
        assert_eq!(attrs.grace_reach(), 37);

        let attrs = ActorAttributes { might_grace_shift: 0, ..base };
        assert_eq!(attrs.might_reach(), 87);
        assert_eq!(attrs.might(), 75);  // abs(-50 + 0 - 25) = abs(-75) = 75
        assert_eq!(attrs.grace(), 25);  // 25 + 0 = 25
        assert_eq!(attrs.grace_reach(), 37);

        let attrs = ActorAttributes { might_grace_shift: 10, ..base };
        assert_eq!(attrs.might_reach(), 87);
        assert_eq!(attrs.might(), 70);  // abs(-50 + 10/2 - 25) = abs(-70) = 70
        assert_eq!(attrs.grace(), 30);  // 25 + 10/2 = 25 + 5 = 30
        assert_eq!(attrs.grace_reach(), 37);

        let attrs = ActorAttributes { might_grace_shift: 25, ..base };
        assert_eq!(attrs.might_reach(), 87);
        assert_eq!(attrs.might(), 63);  // abs(-50 + 25/2 - 25) = abs(-50 + 12 - 25) = abs(-63) = 63
        assert_eq!(attrs.grace(), 37);  // 25 + 25/2 = 25 + 12 = 37
        assert_eq!(attrs.grace_reach(), 37);
    }

    #[test]
    fn test_perfectly_balanced_attributes() {
        // Perfectly balanced: axis=0, spectrum=20: 0A/20S
        // might_reach = abs(0 - 20*1.5) = abs(-30) = 30
        // grace_reach = abs(0 + 20*1.5) = abs(30) = 30
        let base = ActorAttributes {
            might_grace_axis: 0,
            might_grace_spectrum: 20,
            ..Default::default()
        };

        // Shift fully toward might
        let attrs = ActorAttributes { might_grace_shift: -20, ..base };
        assert_eq!(attrs.might_reach(), 30);  // abs(0 - 30) = 30
        assert_eq!(attrs.might(), 30);  // abs(0 + (-20)/2 - 20) = abs(-30) = 30
        assert_eq!(attrs.grace(), 10);  // abs(0 + (-20)/2 + 20) = abs(10) = 10
        assert_eq!(attrs.grace_reach(), 30);  // abs(0 + 30) = 30

        // Shift partially toward might
        let attrs = ActorAttributes { might_grace_shift: -10, ..base };
        assert_eq!(attrs.might_reach(), 30);
        assert_eq!(attrs.might(), 25);  // abs(0 + (-10)/2 - 20) = abs(-25) = 25
        assert_eq!(attrs.grace(), 15);  // abs(0 + (-10)/2 + 20) = abs(15) = 15
        assert_eq!(attrs.grace_reach(), 30);

        // No shift (perfectly centered)
        let attrs = ActorAttributes { might_grace_shift: 0, ..base };
        assert_eq!(attrs.might_reach(), 30);
        assert_eq!(attrs.might(), 20);  // abs(0 + 0 - 20) = abs(-20) = 20
        assert_eq!(attrs.grace(), 20);  // abs(0 + 0 + 20) = abs(20) = 20
        assert_eq!(attrs.grace_reach(), 30);

        // Shift partially toward grace
        let attrs = ActorAttributes { might_grace_shift: 10, ..base };
        assert_eq!(attrs.might_reach(), 30);
        assert_eq!(attrs.might(), 15);  // abs(0 + 10/2 - 20) = abs(-15) = 15
        assert_eq!(attrs.grace(), 25);  // abs(0 + 10/2 + 20) = abs(25) = 25
        assert_eq!(attrs.grace_reach(), 30);

        // Shift fully toward grace
        let attrs = ActorAttributes { might_grace_shift: 20, ..base };
        assert_eq!(attrs.might_reach(), 30);
        assert_eq!(attrs.might(), 10);  // abs(0 + 20/2 - 20) = abs(-10) = 10
        assert_eq!(attrs.grace(), 30);  // abs(0 + 20/2 + 20) = abs(30) = 30
        assert_eq!(attrs.grace_reach(), 30);
    }

    #[test]
    fn test_narrow_spectrum_attributes() {
        // axis=-80, spectrum=10: -80A/10S (on might side)
        // might_reach = abs(-80 - 10*1.5) = abs(-80 - 15) = 95
        // grace_reach = 10 * 1.5 = 15
        let base = ActorAttributes {
            might_grace_axis: -80,
            might_grace_spectrum: 10,
            ..Default::default()
        };

        let attrs = ActorAttributes { might_grace_shift: -10, ..base };
        assert_eq!(attrs.might_reach(), 95);  // abs(-80 - 15) = 95
        assert_eq!(attrs.might(), 95);  // abs(-80 + (-10)/2 - 10) = abs(-95) = 95
        assert_eq!(attrs.grace(), 5);  // 10 + (-10)/2 = 10 - 5 = 5
        assert_eq!(attrs.grace_reach(), 15);  // 10 * 1.5 = 15

        let attrs = ActorAttributes { might_grace_shift: -5, ..base };
        assert_eq!(attrs.might_reach(), 95);
        assert_eq!(attrs.might(), 92);  // abs(-80 + (-5)/2 - 10) = abs(-92) = 92
        assert_eq!(attrs.grace(), 8);  // 10 + (-5)/2 = 10 + (-2) = 8 (integer division)
        assert_eq!(attrs.grace_reach(), 15);

        let attrs = ActorAttributes { might_grace_shift: 0, ..base };
        assert_eq!(attrs.might_reach(), 95);
        assert_eq!(attrs.might(), 90);  // abs(-80 + 0 - 10) = abs(-90) = 90
        assert_eq!(attrs.grace(), 10);  // 10 + 0 = 10
        assert_eq!(attrs.grace_reach(), 15);

        let attrs = ActorAttributes { might_grace_shift: 5, ..base };
        assert_eq!(attrs.might_reach(), 95);
        assert_eq!(attrs.might(), 88);  // abs(-80 + 5/2 - 10) = abs(-88) = 88
        assert_eq!(attrs.grace(), 12);  // 10 + 5/2 = 10 + 2 = 12
        assert_eq!(attrs.grace_reach(), 15);

        let attrs = ActorAttributes { might_grace_shift: 10, ..base };
        assert_eq!(attrs.might_reach(), 95);
        assert_eq!(attrs.might(), 85);  // abs(-80 + 10/2 - 10) = abs(-85) = 85
        assert_eq!(attrs.grace(), 15);  // 10 + 10/2 = 10 + 5 = 15
        assert_eq!(attrs.grace_reach(), 15);
    }

    // ===== MOVEMENT SPEED TESTS (ADR-010 Phase 2) =====

    #[test]
    fn test_movement_speed_formula_baseline() {
        // Grace = 0 (parity) should give baseline speed (100%)
        // Formula: max(75, 100 + (grace / 2))
        // Grace = 0: max(75, 100 + 0) = 100
        // Speed multiplier: 100 / 100 = 1.0
        // Final speed: 0.005 * 1.0 = 0.005
        let attrs = ActorAttributes {
            might_grace_axis: 0,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            ..Default::default()
        };

        let speed = attrs.movement_speed();
        assert!(
            (speed - 0.005).abs() < 0.0001,
            "Grace 0 should give baseline speed 0.005, got {}",
            speed
        );
    }

    #[test]
    fn test_movement_speed_formula_grace_100() {
        // Grace = 100 (Grace specialist) should give +50% speed
        // Formula: max(75, 100 + (100 / 2)) = max(75, 150) = 150
        // Speed multiplier: 150 / 100 = 1.5
        // Final speed: 0.005 * 1.5 = 0.0075
        let attrs = ActorAttributes {
            might_grace_axis: 100,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            ..Default::default()
        };

        let speed = attrs.movement_speed();
        assert!(
            (speed - 0.0075).abs() < 0.0001,
            "Grace 100 should give 150% speed (0.0075), got {}",
            speed
        );
    }

    #[test]
    fn test_movement_speed_formula_grace_neg100() {
        // Grace = -100 (Might specialist) should be clamped at 75% speed
        // Formula: max(75, 100 + (-100 / 2)) = max(75, 50) = 75
        // Speed multiplier: 75 / 100 = 0.75
        // Final speed: 0.005 * 0.75 = 0.00375
        let attrs = ActorAttributes {
            might_grace_axis: -100,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            ..Default::default()
        };

        let speed = attrs.movement_speed();
        assert!(
            (speed - 0.00375).abs() < 0.0001,
            "Grace -100 should be clamped at 75% speed (0.00375), got {}",
            speed
        );
    }

    #[test]
    fn test_movement_speed_formula_grace_50() {
        // Grace = 50 should give +25% speed
        // Formula: max(75, 100 + (50 / 2)) = max(75, 125) = 125
        // Speed multiplier: 125 / 100 = 1.25
        // Final speed: 0.005 * 1.25 = 0.00625
        let attrs = ActorAttributes {
            might_grace_axis: 50,
            might_grace_spectrum: 0,
            might_grace_shift: 0,
            ..Default::default()
        };

        let speed = attrs.movement_speed();
        assert!(
            (speed - 0.00625).abs() < 0.0001,
            "Grace 50 should give 125% speed (0.00625), got {}",
            speed
        );
    }

    #[test]
    fn test_movement_speed_with_axis_and_shift() {
        // Test that shift affects movement speed via might_grace()
        // axis = 80, shift = 20 -> might_grace() = 100 (clamped)
        let attrs = ActorAttributes {
            might_grace_axis: 80,
            might_grace_spectrum: 30,
            might_grace_shift: 20,
            ..Default::default()
        };

        // might_grace() = clamp(80 + 20, -100, 100) = 100
        let speed = attrs.movement_speed();
        assert!(
            (speed - 0.0075).abs() < 0.0001,
            "Grace 100 (via axis+shift) should give 150% speed (0.0075), got {}",
            speed
        );
    }

}