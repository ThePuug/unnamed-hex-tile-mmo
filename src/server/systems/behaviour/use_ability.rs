use bevy::prelude::*;
use bevy_behave::prelude::*;

use crate::common::{
    components::{Loc, heading::Heading, gcd::Gcd},
    message::{Event, Try, AbilityType},
    systems::targeting::is_in_facing_cone,
};
use crate::server::systems::behaviour::Target;

/// UseAbilityIfAdjacent behavior node - emits Try::UseAbility if conditions met
///
/// From ADR-006:
/// - Checks GCD, distance==1, and facing cone (60° from ADR-004)
/// - Emits Try::UseAbility event for server to process
/// - Fails gracefully if conditions not met (node retries next behavior tree loop)
/// - This is the critical node that makes NPCs actually attack
#[derive(Clone, Component, Copy, Debug)]
pub struct UseAbilityIfAdjacent {
    pub ability: AbilityType,
}

pub fn use_ability_if_adjacent(
    mut commands: Commands,
    mut writer: EventWriter<Try>,
    time: Res<Time>,
    mut query: Query<(&UseAbilityIfAdjacent, &BehaveCtx)>,
    q_npc: Query<(Entity, &Loc, &Heading, Option<&Target>, Option<&Gcd>)>,
    q_target_loc: Query<&Loc>,
) {
    for (&node, &ctx) in &mut query {
        let npc_entity = ctx.target_entity();

        // Get NPC's state
        let Ok((npc_ent, npc_loc, npc_heading, target_opt, gcd_opt)) = q_npc.get(npc_entity) else {
            commands.trigger(ctx.failure());
            continue;
        };

        // Check if NPC has a Target
        let Some(target) = target_opt else {
            commands.trigger(ctx.failure());
            continue;
        };

        // Check GCD cooldown
        if let Some(gcd) = gcd_opt {
            if gcd.is_active(time.elapsed()) {
                // GCD active, fail (node will retry on next behavior tree tick)
                commands.trigger(ctx.failure());
                continue;
            }
        }

        // Unwrap Target (Option<Entity>)
        let Some(target_ent) = **target else {
            // No target entity set, fail
            commands.trigger(ctx.failure());
            continue;
        };

        // Get Target's location
        let Ok(target_loc) = q_target_loc.get(target_ent) else {
            // Target entity missing Loc, fail
            commands.trigger(ctx.failure());
            continue;
        };

        // Check if adjacent (distance == 1)
        let distance = npc_loc.distance(target_loc);
        if distance != 1 {
            // Not adjacent, fail
            commands.trigger(ctx.failure());
            continue;
        }

        // Check if facing target (within 60° cone)
        if !is_in_facing_cone(*npc_heading, *npc_loc, *target_loc) {
            // Not facing, fail (FaceTarget node should run before this)
            commands.trigger(ctx.failure());
            continue;
        }

        // All conditions met: Emit ability usage
        writer.write(Try {
            event: Event::UseAbility {
                ent: npc_ent,
                ability: node.ability,
                target_loc: Some(**target_loc), // Send target hex for validation
            },
        });

        // Success (ability emitted, server will process)
        commands.trigger(ctx.success());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;

    #[test]
    fn test_facing_cone_directly_ahead() {
        // Heading east, target directly east
        let heading = Heading::new(Qrz { q: 1, r: 0, z: -1 });
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target = Loc::new(Qrz { q: 5, r: 0, z: -5 });

        assert!(is_in_facing_cone(heading, caster, target));
    }

    #[test]
    fn test_facing_cone_adjacent_east() {
        // Heading east, target 1 tile east
        let heading = Heading::new(Qrz { q: 1, r: 0, z: -1 });
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target = Loc::new(Qrz { q: 1, r: 0, z: -1 });

        assert!(is_in_facing_cone(heading, caster, target));
    }

    #[test]
    fn test_facing_cone_behind() {
        // Heading east, target directly west (behind)
        let heading = Heading::new(Qrz { q: 1, r: 0, z: -1 });
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target = Loc::new(Qrz { q: -5, r: 0, z: 5 });

        assert!(!is_in_facing_cone(heading, caster, target));
    }

    #[test]
    fn test_facing_cone_perpendicular() {
        // Heading east, target to the north (perpendicular)
        let heading = Heading::new(Qrz { q: 1, r: 0, z: -1 });
        let caster = Loc::new(Qrz { q: 0, r: 0, z: 0 });
        let target = Loc::new(Qrz { q: 0, r: -5, z: 5 });

        // This should fail because north is perpendicular to east
        assert!(!is_in_facing_cone(heading, caster, target));
    }
}
