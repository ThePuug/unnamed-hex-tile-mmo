use bevy::prelude::*;

use crate::common::{
    components::recovery::{GlobalRecovery, get_ability_recovery_duration},
    message::{Do, Event as GameEvent, AbilityType},
    systems::combat::synergies::apply_synergies,
};

/// Client-side handler for Do UseAbility (ADR-012)
/// Server broadcasts when ability succeeds, client applies recovery/synergies locally
pub fn handle_ability_used(
    mut commands: Commands,
    mut do_reader: EventReader<Do>,
) {
    for event in do_reader.read() {
        let Do { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
            continue;
        };

        // Skip AutoAttack - it has its own timer and doesn't use recovery system
        if *ability == AbilityType::AutoAttack {
            continue;
        }

        // Insert GlobalRecovery component (same as server)
        // Only insert if entity exists (may have been evicted)
        let recovery_duration = get_ability_recovery_duration(*ability);
        let recovery = GlobalRecovery::new(recovery_duration, *ability);
        if let Ok(mut entity_cmd) = commands.get_entity(*ent) {
            entity_cmd.insert(recovery);

            // Apply synergies (same as server)
            apply_synergies(*ent, *ability, &recovery, &mut commands);
        }
    }
}
