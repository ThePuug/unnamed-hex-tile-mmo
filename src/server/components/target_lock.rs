use bevy::prelude::*;
use crate::common::components::Loc;

/// Target lock component for sticky target acquisition
/// Prevents NPCs from switching targets until the current target becomes invalid
#[derive(Component, Clone, Copy, Debug)]
pub struct TargetLock {
    pub locked_target: Entity,
    pub max_chase_distance: i16,    // Leash distance (0 = infinite)
}

impl TargetLock {
    pub fn new(target: Entity, leash: i16) -> Self {
        Self {
            locked_target: target,
            max_chase_distance: leash,
        }
    }

    /// Check if the locked target is still valid
    /// Returns false if:
    /// - target_loc is None (entity despawned or missing Loc)
    /// - target is beyond max_chase_distance (unless max_chase_distance == 0)
    pub fn is_target_valid(
        &self,
        target_loc: Option<&Loc>,
        npc_loc: &Loc,
    ) -> bool {
        match target_loc {
            Some(loc) => {
                if self.max_chase_distance == 0 {
                    true  // No leash - always valid if entity exists
                } else {
                    npc_loc.distance(loc) <= self.max_chase_distance
                }
            }
            None => false,  // Target entity despawned or missing Loc
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;

    #[test]
    fn test_target_lock_new() {
        let target_ent = Entity::from_raw(42);
        let lock = TargetLock::new(target_ent, 30);

        assert_eq!(lock.locked_target.index(), 42);
        assert_eq!(lock.max_chase_distance, 30);
    }

    #[test]
    fn test_target_valid_within_leash() {
        let target_ent = Entity::from_raw(42);
        let lock = TargetLock::new(target_ent, 30);

        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target_loc = Loc::new(Qrz { q: 10, r: -10, z: 0 });  // Distance 10

        assert!(lock.is_target_valid(Some(&target_loc), &npc_loc));
    }

    #[test]
    fn test_target_invalid_beyond_leash() {
        let target_ent = Entity::from_raw(42);
        let lock = TargetLock::new(target_ent, 30);

        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target_loc = Loc::new(Qrz { q: 50, r: -50, z: 0 });  // Distance 50 > 30

        assert!(!lock.is_target_valid(Some(&target_loc), &npc_loc));
    }

    #[test]
    fn test_target_valid_exactly_at_leash() {
        let target_ent = Entity::from_raw(42);
        let lock = TargetLock::new(target_ent, 30);

        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target_loc = Loc::new(Qrz { q: 30, r: -30, z: 0 });  // Distance exactly 30

        assert!(lock.is_target_valid(Some(&target_loc), &npc_loc));
    }

    #[test]
    fn test_target_invalid_when_despawned() {
        let target_ent = Entity::from_raw(42);
        let lock = TargetLock::new(target_ent, 30);

        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // None means entity despawned or missing Loc
        assert!(!lock.is_target_valid(None, &npc_loc));
    }

    #[test]
    fn test_infinite_leash_always_valid() {
        let target_ent = Entity::from_raw(42);
        let lock = TargetLock::new(target_ent, 0);  // 0 = infinite leash

        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target_loc = Loc::new(Qrz { q: 1000, r: -1000, z: 0 });  // Very far

        assert!(lock.is_target_valid(Some(&target_loc), &npc_loc));
    }

    #[test]
    fn test_infinite_leash_invalid_when_despawned() {
        let target_ent = Entity::from_raw(42);
        let lock = TargetLock::new(target_ent, 0);  // 0 = infinite leash

        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Even with infinite leash, despawned target is invalid
        assert!(!lock.is_target_valid(None, &npc_loc));
    }

    #[test]
    fn test_target_valid_with_vertical_distance() {
        let target_ent = Entity::from_raw(42);
        let lock = TargetLock::new(target_ent, 50);

        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        // Target at same horizontal position but high Z
        // Flat distance 0, Z diff 30, total distance 30
        let target_loc = Loc::new(Qrz { q: 0, r: 0, z: 30 });

        assert!(lock.is_target_valid(Some(&target_loc), &npc_loc));
    }

    #[test]
    fn test_target_invalid_with_vertical_distance_beyond_leash() {
        let target_ent = Entity::from_raw(42);
        let lock = TargetLock::new(target_ent, 25);

        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        // Target at same horizontal position but high Z
        // Flat distance 0, Z diff 30, total distance 30 > 25
        let target_loc = Loc::new(Qrz { q: 0, r: 0, z: 30 });

        assert!(!lock.is_target_valid(Some(&target_loc), &npc_loc));
    }
}
