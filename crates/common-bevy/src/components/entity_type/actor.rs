use serde::{Deserialize, Serialize};

use crate::archetype::EnemyArchetype;

/// Who an actor is, which names it and picks the body it is drawn with: a
/// player, an NPC of an archetype, the nest an archetype's pack lives
/// in, or a settler, who builds a hub and fights no one. A nest has no
/// body: its den is drawn as the world's, and it stands where that does,
/// never turning and never moved.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ActorIdentity {
    Player,
    Npc(EnemyArchetype),
    Nest(EnemyArchetype),
    Settler,
}

/// The share of a run a settler walks at, on foot or riding in.
pub const SETTLER_PACE: f32 = 0.5;

/// The body every rider sits on.
pub const HORSE: &str = "horse";

impl ActorIdentity {
    /// The pace this actor moves at where its status leaves it `status`:
    /// a settler walks at [`SETTLER_PACE`] whatever its status, its horse
    /// included. The server steers by this and a client simulates by it;
    /// a pace only one of them knows is drawn running ahead and pulled
    /// back at every tile.
    pub fn pace(&self, status: f32) -> f32 {
        match self {
            ActorIdentity::Settler => SETTLER_PACE,
            _ => status,
        }
    }

    /// The body it is drawn with, by the stem of its rig under `actors/`:
    /// identity decides it, the triumvirate shapes combat only. A nest has
    /// none.
    pub fn body(&self) -> Option<&'static str> {
        Some(match self {
            ActorIdentity::Player | ActorIdentity::Settler | ActorIdentity::Npc(EnemyArchetype::Defender) => "player",
            ActorIdentity::Npc(EnemyArchetype::Berserker) => "dog",
            ActorIdentity::Npc(EnemyArchetype::Flanker) => "sprite",
            ActorIdentity::Npc(EnemyArchetype::Juggernaut) => "juggernaut",
            ActorIdentity::Npc(EnemyArchetype::Skirmisher) => "grasshopper",
            ActorIdentity::Npc(EnemyArchetype::Ambusher) => "crocodile",
            ActorIdentity::Nest(_) => return None,
        })
    }

    /// How deep water must stand over the ground to float this actor, in
    /// world units: its body's waterline, or its horse's while `mounted`.
    /// Infinite where the body declares none, which walks the bed at any
    /// depth, as a nest's absent body does.
    pub fn draft(&self, mounted: bool) -> f32 {
        let body = if mounted { Some(HORSE) } else { self.body() };
        body.and_then(common::body::waterline).unwrap_or(f32::INFINITY)
    }

    /// The draft of the actor `typ` names, under `status`
    /// ([`ActorIdentity::draft`]); infinite for what is no actor.
    pub fn draft_of(typ: Option<&crate::components::entity_type::EntityType>, status: Option<&crate::components::status::Status>) -> f32 {
        match typ {
            Some(crate::components::entity_type::EntityType::Actor(actor)) => actor.identity.draft(status.is_some_and(|s| s.mounted)),
            _ => f32::INFINITY,
        }
    }

    /// Get human-readable display name for this actor
    pub fn display_name(&self) -> &'static str {
        match self {
            ActorIdentity::Player => "Player",
            ActorIdentity::Npc(archetype) => archetype.profile().name,
            ActorIdentity::Nest(_) => "Nest",
            ActorIdentity::Settler => "Settler",
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
