// Combat-related systems module
// Consolidates all combat mechanics (state, resources, queues, GCD, damage)

pub mod gcd;
pub mod queue;
pub mod resources;
pub mod state;

// Re-export commonly used items for convenience
// Note: Only re-export items that are actively used by other modules
// to avoid unused import warnings
