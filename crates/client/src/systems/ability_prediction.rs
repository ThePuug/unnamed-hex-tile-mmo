use bevy::prelude::*;

use common_bevy::{
    components::{recovery::{GlobalRecovery, SynergyUnlock}, target::Target},
    message::{Do, Event as GameEvent, AbilityType},
    systems::combat::synergies::{apply_synergies, lockout},
};

/// Client-side handler for Do UseAbility
/// Server broadcasts when ability succeeds, client applies recovery/synergies locally
pub fn handle_ability_used(
    mut commands: Commands,
    mut do_reader: MessageReader<Do>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    target_query: Query<&Target>,
    recovery_query: Query<(Option<&GlobalRecovery>, Option<&SynergyUnlock>)>,
) {
    for event in do_reader.read() {
        let Do { event: GameEvent::UseAbility { ent, ability, target: _ } } = event else {
            continue;
        };

        // Skip AutoAttack - it has its own timer and doesn't use recovery system
        if *ability == AbilityType::AutoAttack {
            continue;
        }

        // Insert GlobalRecovery component (same as server, carrying what an
        // early follow-up skipped). Only insert if entity exists (may have been evicted)
        let (prior, offer) = recovery_query.get(*ent).map_or((None, None), |(recovery, offer)| (recovery.copied(), offer.copied()));
        let recovery = lockout(*ability, prior.as_ref(), offer.as_ref());
        if let Ok(mut entity_cmd) = commands.get_entity(*ent) {
            entity_cmd.insert(recovery);

            // Apply synergies (optimistic client-side,)
            // Contest: player's flow vs target's reflex
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
