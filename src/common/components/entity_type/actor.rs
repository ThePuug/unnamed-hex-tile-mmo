use serde::{Deserialize, Serialize};

/// Actor identity - separates combat behavior (triumvirate) from visual/display identity
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ActorIdentity {
    Player,
    Npc(NpcType),
}

impl ActorIdentity {
    /// Get human-readable display name for this actor
    pub fn display_name(&self) -> &'static str {
        match self {
            ActorIdentity::Player => "Player",
            ActorIdentity::Npc(npc_type) => npc_type.display_name(),
        }
    }
}

/// NPC type variants - each represents a distinct enemy or NPC identity
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum NpcType {
    WildDog,
    ForestSprite,
}

impl NpcType {
    /// Get human-readable display name for this NPC type
    pub fn display_name(&self) -> &'static str {
        match self {
            NpcType::WildDog => "Wild Dog",
            NpcType::ForestSprite => "Forest Sprite",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forest_sprite_display_name() {
        assert_eq!(NpcType::ForestSprite.display_name(), "Forest Sprite");
    }

    #[test]
    fn test_forest_sprite_actor_identity() {
        let identity = ActorIdentity::Npc(NpcType::ForestSprite);
        assert_eq!(identity.display_name(), "Forest Sprite");
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

    /// Get display name for this origin
    pub fn display_name(&self) -> &'static str {
        match self {
            Origin::Evolved => "Evolved",
            Origin::Synthetic => "Synthetic",
            Origin::Essential => "Essential",
            Origin::Corrupted => "Corrupted",
            Origin::Mythic => "Mythic",
            Origin::Forgotten => "Forgotten",
            Origin::Indiscernible => "Indiscernible",
        }
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Approach {
    Direct, // simple, straightforward, honest
    Distant, // attacks from safety, indirect, aloof
    Ambushing, // cunning, stealthy, untrustworthy
    Patient, // calculating, immobile, consistent
    Binding, // controlling, dominant, restrictive
    Evasive, // reactive, slippery, indecisive
    Overwhelming, // relentless, unstoppable, inescapable
}

impl Approach {
    /// Get display name for this approach
    pub fn display_name(&self) -> &'static str {
        match self {
            Approach::Direct => "Direct",
            Approach::Distant => "Distant",
            Approach::Ambushing => "Ambushing",
            Approach::Patient => "Patient",
            Approach::Binding => "Binding",
            Approach::Evasive => "Evasive",
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
