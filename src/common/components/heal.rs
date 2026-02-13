use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Represents a healing event that needs to be applied (SOW-021 Phase 3)
#[derive(Component, Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PendingHeal {
    pub source: Entity,      // Who cast the heal
    pub target: Entity,      // Who receives the heal
    pub amount: f32,          // Base healing amount (before Dominance reduction)
    pub heal_type: HealType,  // Physical/Magic for future expansion
}

/// Type of healing for future expansion
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HealType {
    Physical, // Vitality-based healing (bandages, potions)
    Magic,    // Focus-based healing (spells, regeneration)
}
