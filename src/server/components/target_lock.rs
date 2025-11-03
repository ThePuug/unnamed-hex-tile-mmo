use bevy::prelude::*;
use crate::common::components::Loc;

/// Target lock component for sticky target acquisition
/// Prevents NPCs from switching targets until the current target becomes invalid
#[derive(Component, Clone, Copy, Debug)]
pub struct TargetLock {
    pub locked_target: Entity,
    pub max_chase_distance: i16,    // Leash distance (0 = infinite)
    pub origin: Loc,                // Where the NPC was when lock was created (leash anchor point)
}

impl TargetLock {
    pub fn new(target: Entity, leash: i16, origin: Loc) -> Self {
        Self {
            locked_target: target,
            max_chase_distance: leash,
            origin,
        }
    }

    /// Check if the locked target is still valid
    /// Returns false if:
    /// - target_loc is None (entity despawned or missing Loc)
    /// - NPC has strayed beyond max_chase_distance from origin (unless max_chase_distance == 0)
    pub fn is_target_valid(
        &self,
        target_loc: Option<&Loc>,
        npc_loc: &Loc,
    ) -> bool {
        match target_loc {
            Some(_loc) => {
                if self.max_chase_distance == 0 {
                    true  // No leash - always valid if entity exists
                } else {
                    // Check if NPC is still within leash distance of origin (spawn point)
                    self.origin.distance(npc_loc) <= self.max_chase_distance
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
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let lock = TargetLock::new(target_ent, 30, origin);

        assert_eq!(lock.locked_target.index(), 42);
        assert_eq!(lock.max_chase_distance, 30);
        assert_eq!(lock.origin, origin);
    }

    #[test]
    fn test_target_valid_within_leash() {
        let target_ent = Entity::from_raw(42);
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let lock = TargetLock::new(target_ent, 30, origin);

        // NPC moved 10 tiles from origin (within 30 tile leash)
        let npc_loc = Loc::new(Qrz { q: 10, r: -10, z: 0 });
        let target_loc = Loc::new(Qrz { q: 50, r: -50, z: 0 });  // Target far away

        assert!(lock.is_target_valid(Some(&target_loc), &npc_loc));
    }

    #[test]
    fn test_target_invalid_beyond_leash() {
        let target_ent = Entity::from_raw(42);
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let lock = TargetLock::new(target_ent, 30, origin);

        // NPC moved 50 tiles from origin (beyond 30 tile leash)
        let npc_loc = Loc::new(Qrz { q: 50, r: -50, z: 0 });
        let target_loc = Loc::new(Qrz { q: 51, r: -51, z: 0 });  // Target location irrelevant

        assert!(!lock.is_target_valid(Some(&target_loc), &npc_loc));
    }

    #[test]
    fn test_target_valid_exactly_at_leash() {
        let target_ent = Entity::from_raw(42);
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let lock = TargetLock::new(target_ent, 30, origin);

        // NPC moved exactly 30 tiles from origin
        let npc_loc = Loc::new(Qrz { q: 30, r: -30, z: 0 });
        let target_loc = Loc::new(Qrz { q: 100, r: -100, z: 0 });  // Target location irrelevant

        assert!(lock.is_target_valid(Some(&target_loc), &npc_loc));
    }

    #[test]
    fn test_target_invalid_when_despawned() {
        let target_ent = Entity::from_raw(42);
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let lock = TargetLock::new(target_ent, 30, origin);

        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // None means entity despawned or missing Loc
        assert!(!lock.is_target_valid(None, &npc_loc));
    }

    #[test]
    fn test_infinite_leash_always_valid() {
        let target_ent = Entity::from_raw(42);
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let lock = TargetLock::new(target_ent, 0, origin);  // 0 = infinite leash

        // NPC moved very far from origin
        let npc_loc = Loc::new(Qrz { q: 1000, r: -1000, z: 0 });
        let target_loc = Loc::new(Qrz { q: 1001, r: -1001, z: 0 });

        assert!(lock.is_target_valid(Some(&target_loc), &npc_loc));
    }

    #[test]
    fn test_infinite_leash_invalid_when_despawned() {
        let target_ent = Entity::from_raw(42);
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let lock = TargetLock::new(target_ent, 0, origin);  // 0 = infinite leash

        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 0 });

        // Even with infinite leash, despawned target is invalid
        assert!(!lock.is_target_valid(None, &npc_loc));
    }

    #[test]
    fn test_target_valid_with_vertical_distance() {
        let target_ent = Entity::from_raw(42);
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let lock = TargetLock::new(target_ent, 50, origin);

        // NPC moved vertically from origin (distance 30 from origin)
        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 30 });
        let target_loc = Loc::new(Qrz { q: 10, r: -10, z: 30 });

        assert!(lock.is_target_valid(Some(&target_loc), &npc_loc));
    }

    #[test]
    fn test_target_invalid_with_vertical_distance_beyond_leash() {
        let target_ent = Entity::from_raw(42);
        let origin = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let lock = TargetLock::new(target_ent, 25, origin);

        // NPC moved vertically from origin (distance 30 from origin, beyond 25 tile leash)
        let npc_loc = Loc::new(Qrz { q: 0, r: 0, z: 30 });
        let target_loc = Loc::new(Qrz { q: 0, r: 0, z: 30 });

        assert!(!lock.is_target_valid(Some(&target_loc), &npc_loc));
    }
}
