use bevy::prelude::*;
use crate::{
    common::{
        components::{entity_type::*, heading::*, resources::*, Loc, reaction_queue::DamageType},
        message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
        plugins::nntree::*,
        systems::{targeting::*, combat::gcd::GcdType},
    },
    server::systems::combat::abilities::TriggerGcd,
};

/// Handle Overpower ability (W key)
/// - 40 stamina cost
/// - 80 base damage
/// - Melee range (adjacent hex)
/// - Triggers Attack GCD
pub fn handle_overpower(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    entity_query: Query<(&EntityType, &Loc, Option<&crate::common::components::behaviour::PlayerControlled>)>,
    loc_heading_query: Query<(&Loc, &Heading)>,
    mut stamina_query: Query<&mut Stamina>,
    respawn_query: Query<&RespawnTimer>,
    nntree: Res<NNTree>,
    mut writer: EventWriter<Do>,
    mut gcd_writer: EventWriter<TriggerGcd>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
            continue;
        };

        // Filter for Overpower only
        let Some(AbilityType::Overpower) = (ability == &AbilityType::Overpower).then_some(ability) else {
            continue;
        };

        // Get caster's location and heading
        let Ok((caster_loc, caster_heading)) = loc_heading_query.get(*ent) else {
            continue;
        };

        // Determine if caster is a player (for asymmetric targeting)
        let caster_is_player = entity_query
            .get(*ent)
            .ok()
            .and_then(|(_, _, pc_opt)| pc_opt)
            .is_some();

        // Use targeting system to select target (asymmetric: players attack NPCs, NPCs attack players)
        let target_opt = select_target(
            *ent, // caster entity
            *caster_loc,
            *caster_heading,
            None, // No tier lock in MVP
            &nntree,
            |target_ent| {
                // Skip entities with RespawnTimer (dead players)
                if respawn_query.get(target_ent).is_ok() {
                    return None;
                }
                entity_query.get(target_ent).ok().and_then(|(et, _, player_controlled_opt)| {
                    let target_is_player = player_controlled_opt.is_some();

                    // Asymmetric targeting: can only attack entities on opposite "team"
                    // Players attack NPCs, NPCs attack players (no friendly fire)
                    if caster_is_player != target_is_player {
                        Some(*et)
                    } else {
                        None  // Same team - no friendly fire
                    }
                })
            },
        );

        let Some(target_ent) = target_opt else {
            // No valid target in facing cone
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        };

        // Overpower is melee only - check distance (must be adjacent = distance 1)
        let Some(target_loc) = entity_query.get(target_ent).ok().map(|(_, loc, _)| *loc) else {
            continue;
        };

        let distance = caster_loc.flat_distance(&target_loc) as u32;

        if distance > 1 {
            // Target is too far for melee attack
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        // Check stamina cost (40)
        let stamina_cost = 40.0;
        let base_damage = 80.0;

        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        if stamina.state < stamina_cost {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::InsufficientStamina,
                },
            });
            continue;
        }

        stamina.state -= stamina_cost;
        stamina.step = stamina.state;

        // Broadcast updated stamina
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: crate::common::message::Component::Stamina(*stamina),
            },
        });

        // Emit DealDamage event
        commands.trigger_targets(
            Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target: target_ent,
                    base_damage,
                    damage_type: DamageType::Physical,
                },
            },
            target_ent,
        );

        // Request GCD trigger
        gcd_writer.write(TriggerGcd {
            ent: *ent,
            typ: GcdType::Attack,
        });
    }
}
