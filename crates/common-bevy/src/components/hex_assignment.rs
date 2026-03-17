//! # Hex Assignment Components (SOW-018 Phase 2)
//!
//! Coordinates NPC positioning around player targets by assigning unique
//! approach hexes to each melee NPC in an engagement.

use bevy::prelude::*;
use bevy::platform::collections::HashMap;
use qrz::Qrz;

/// Engagement-level hex assignment map.
///
/// Tracks which hex each NPC is assigned to approach, and the last known
/// player tile for change detection (reassignment triggers on tile change).
#[derive(Component, Debug, Clone)]
pub struct HexAssignment {
    /// NPC entity → assigned approach hex
    pub assignments: HashMap<Entity, Qrz>,
    /// Last known player tile (for change detection)
    pub last_player_tile: Option<Qrz>,
    /// The player entity being targeted
    pub target_player: Option<Entity>,
}

impl Default for HexAssignment {
    fn default() -> Self {
        Self {
            assignments: HashMap::default(),
            last_player_tile: None,
            target_player: None,
        }
    }
}

impl HexAssignment {
    /// Get the assigned hex for an NPC, if any
    pub fn get(&self, npc: Entity) -> Option<Qrz> {
        self.assignments.get(&npc).copied()
    }

    /// Remove an NPC's assignment (e.g., on death)
    pub fn remove(&mut self, npc: Entity) {
        self.assignments.remove(&npc);
    }
}

/// Marker component on individual NPCs indicating their assigned approach hex.
///
/// Written by the assignment system, read by chase to determine pathfinding target.
#[derive(Clone, Component, Copy, Debug)]
pub struct AssignedHex(pub Qrz);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_assignment_default_is_empty() {
        let ha = HexAssignment::default();
        assert!(ha.assignments.is_empty());
        assert!(ha.last_player_tile.is_none());
        assert!(ha.target_player.is_none());
    }
}
