use serde::{Deserialize, Serialize};

use crate::archetype::EnemyArchetype;

/// Who an actor is, which names it and picks the body it is drawn with: a
/// player, or an NPC of an archetype.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ActorIdentity {
    Player,
    Npc(EnemyArchetype),
}

impl ActorIdentity {
    /// Get human-readable display name for this actor
    pub fn display_name(&self) -> &'static str {
        match self {
            ActorIdentity::Player => "Player",
            ActorIdentity::Npc(archetype) => archetype.profile().name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_npc_is_named_for_its_archetype() {
        assert_eq!(ActorIdentity::Npc(EnemyArchetype::Flanker).display_name(), "Forest Sprite");
        assert_eq!(ActorIdentity::Player.display_name(), "Player");
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActorImpl {
    pub origin: Origin,
    pub approach: Approach,
    pub resilience: Resilience,
    pub identity: ActorIdentity,
}

impl ActorImpl {
    pub fn new(origin: Origin, approach: Approach, resilience: Resilience, identity: ActorIdentity) -> Self {
        ActorImpl { origin, approach, resilience, identity }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Origin {
    Evolved,       // Product of natural selection and biological processes
    Synthetic,     // Crafted by artificial means or intelligent design
    Essential,     // Pure manifestation of fundamental forces or concepts
    Corrupted,     // Twisted, blighted, perverted from original form
    Mythic,        // Born from legend, collective belief, and remembered stories
    Forgotten,     // Ancient beings erased from memory, lost to time
    Indiscernible, // Origin cannot be traced or categorized
}

impl Origin {
    /// Get color representing this origin (for UI display)
    pub fn color(&self) -> (f32, f32, f32) {
        match self {
            Origin::Evolved => (0.4, 0.8, 0.4),       // Green - natural, biological
            Origin::Synthetic => (0.7, 0.7, 0.9),     // Light blue - technological
            Origin::Essential => (0.9, 0.9, 1.0),     // Bright white/blue - pure, elemental
            Origin::Corrupted => (0.6, 0.3, 0.6),     // Dark purple - twisted, blighted
            Origin::Mythic => (1.0, 0.8, 0.3),        // Gold - legendary
            Origin::Forgotten => (0.5, 0.5, 0.6),     // Faded gray/blue - lost to time
            Origin::Indiscernible => (0.7, 0.7, 0.7), // Gray - mysterious, unknowable
        }
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Approach {
    Direct, // simple, straightforward, honest
    Oblique, // comes from an angle, where its foe is not looking
    Opportunistic, // seizes the opening a foe gives away
    Vigilant, // watchful, gives no opening
    Binding, // controlling, dominant, restrictive
    Fluid, // keeps the fight moving, never lets it settle
    Overwhelming, // relentless, unstoppable, inescapable
}

impl Approach {
    /// Get display name for this approach
    pub fn display_name(&self) -> &'static str {
        match self {
            Approach::Direct => "Direct",
            Approach::Oblique => "Oblique",
            Approach::Opportunistic => "Opportunistic",
            Approach::Vigilant => "Vigilant",
            Approach::Binding => "Binding",
            Approach::Fluid => "Fluid",
            Approach::Overwhelming => "Overwhelming",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Resilience {
    Vital,     // Physical endurance, emotional stamina
    Mental,    // Consciousness under duress, intellectual fortitude
    Hardened,  // Physical armor, callused to appeals
    Shielded,  // Magical wards, protected by reputation
    Blessed,   // Divine favor, sustained by conviction
    Primal,    // Elemental resistance, raw authenticity
    Eternal,   // Exists across time, cannot be permanently ended
}

impl Resilience {
    /// Get display name for this resilience
    pub fn display_name(&self) -> &'static str {
        match self {
            Resilience::Vital => "Vital",
            Resilience::Mental => "Mental",
            Resilience::Hardened => "Hardened",
            Resilience::Shielded => "Shielded",
            Resilience::Blessed => "Blessed",
            Resilience::Primal => "Primal",
            Resilience::Eternal => "Eternal",
        }
    }
}
