// Combat-related systems module
// Consolidates all combat mechanics (state, resources, queues, GCD, damage, recovery, synergies, scaling)

pub mod damage;
pub mod gcd;
pub mod queue;
pub mod recovery;
pub mod resources;
pub mod scaling;
pub mod state;
pub mod synergies;

// Re-export commonly used items for convenience
// Note: Only re-export items that are actively used by other modules
// to avoid unused import warnings
