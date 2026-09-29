use bevy::prelude::*;

use common_bevy::{
    components::{prepared::Prepared, recovery::{GlobalRecovery, SynergyUnlock}, target::Target},
    message::{Do, Event as GameEvent, AbilityType},
    systems::combat::synergies::{apply_synergies, lockout},
};

/// Client-side handler for Do UseAbility and Do Prepare
/// Server broadcasts when ability succeeds or a reaction is prepared, client
/// applies recovery/synergies locally. A prepared reaction fired starts
/// none: its owner still holds it as the use arrives, the server sending
/// what it holds after.
pub fn handle_ability_used(
    mut commands: Commands,
    mut do_reader: MessageReader<Do>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    target_query: Query<&Target>,
    recovery_query: Query<(Option<&GlobalRecovery>, Option<&SynergyUnlock>)>,
    prepared_query: Query<&Prepared>,
) {
    for event in do_reader.read() {
        let (ent, ability) = match &event.event {
            GameEvent::UseAbility { ent, ability, target: _ } => {
                if prepared_query.get(*ent).is_ok_and(|prepared| prepared.holds(*ability)) {
                    continue;
                }
                (ent, ability)
            }
            GameEvent::Prepare { ent, ability } => (ent, ability),
            _ => continue,
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
