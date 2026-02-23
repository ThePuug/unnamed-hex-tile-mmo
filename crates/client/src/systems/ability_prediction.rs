use bevy::prelude::*;

use common_bevy::{
    components::{recovery::{GlobalRecovery, get_ability_recovery_duration}, target::Target},
    message::{Do, Event as GameEvent, AbilityType},
    systems::combat::synergies::apply_synergies,
};

/// Client-side handler for Do UseAbility (ADR-012)
/// Server broadcasts when ability succeeds, client applies recovery/synergies locally
pub fn handle_ability_used(
    mut commands: Commands,
    mut do_reader: MessageReader<Do>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    target_query: Query<&Target>,
) {
    for event in do_reader.read() {
        let Do { event: GameEvent::UseAbility { ent, ability, target: _ } } = event else {
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

            // Apply synergies (optimistic client-side, SOW-021 Phase 2)
            // Contest: player's finesse vs target's cunning
            if let Ok(attacker_attrs) = attrs_query.get(*ent) {
                let target_result = target_query.get(*ent);
                let target_entity = target_result.ok().and_then(|t| t.entity);
                let defender_attrs_opt = target_entity.and_then(|te| attrs_query.get(te).ok());

                let defender_attrs = defender_attrs_opt.unwrap_or(attacker_attrs);
                apply_synergies(*ent, *ability, &recovery, attacker_attrs, defender_attrs, &mut commands);
            }
        }
    }
}
