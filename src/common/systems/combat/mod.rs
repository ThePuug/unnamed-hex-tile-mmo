// Combat-related systems module
// Consolidates all combat mechanics (state, resources, queues, GCD, damage)

pub mod gcd;
pub mod queue;
pub mod resources;
pub mod state;

// Re-export commonly used items for convenience
pub use gcd::GcdType;
pub use queue::{calculate_queue_capacity, calculate_timer_duration, check_expired_threats, clear_threats, insert_threat};
pub use resources::{calculate_armor, calculate_max_mana, calculate_max_stamina, calculate_mana_regen_rate, calculate_resistance, calculate_stamina_regen_rate, check_death, handle_death, process_respawn, regenerate_resources};
pub use state::{enter_combat, update_combat_state};
