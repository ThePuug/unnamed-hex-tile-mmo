use bevy::prelude::*;
use crate::{
    common::{
        components::{entity_type::*, heading::*, resources::*, Loc},
        message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
        plugins::nntree::*,
        systems::{targeting::*, combat::gcd::GcdType},
    },
    server::systems::combat::abilities::TriggerGcd,
};

/// Handle Knockback ability (E key)
/// - 30 stamina cost
/// - 2 hex range
/// - Pushes target 1 hex away
/// - Triggers Attack GCD
pub fn handle_knockback(
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

        // Filter for Knockback only
        let Some(AbilityType::Knockback) = (ability == &AbilityType::Knockback).then_some(ability) else {
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

        // Use targeting system to find target
        let target_opt = select_target(
            *ent,
            *caster_loc,
            *caster_heading,
            None, // No tier lock
            &nntree,
            |target_ent| {
                // Skip dead players
                if respawn_query.get(target_ent).is_ok() {
                    return None;
                }
                entity_query.get(target_ent).ok().and_then(|(et, _, player_controlled_opt)| {
                    let target_is_player = player_controlled_opt.is_some();
                    // Asymmetric targeting: can only attack entities on opposite "team"
                    if caster_is_player != target_is_player {
                        Some(*et)
                    } else {
                        None
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

        let Some(target_loc) = entity_query.get(target_ent).ok().map(|(_, loc, _)| *loc) else {
            continue;
        };

        // Check range (must be within 2 hexes for Knockback)
        let distance = caster_loc.flat_distance(&target_loc) as u32;

        if distance > 2 || distance < 1 {
            // Target is out of range (or we're on top of them)
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        // Check stamina (30 cost)
        let knockback_stamina_cost = 30.0;
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        if stamina.state < knockback_stamina_cost {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::InsufficientStamina,
                },
            });
            continue;
        }

        // Consume stamina
        stamina.state -= knockback_stamina_cost;
        stamina.step = stamina.state;

        // Broadcast updated stamina
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: crate::common::message::Component::Stamina(*stamina),
            },
        });

        // Calculate push direction: find neighbor of target that's furthest from caster
        let target_neighbors = (*target_loc).neighbors();
        let push_loc = target_neighbors
            .iter()
            .max_by_key(|neighbor_loc| caster_loc.flat_distance(neighbor_loc))
            .copied()
            .unwrap_or(*target_loc); // Fallback to current loc if no neighbors

        // Update target's location (push)
        commands.entity(target_ent).insert(Loc::new(push_loc));

        // Broadcast location update to clients
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: target_ent,
                component: crate::common::message::Component::Loc(Loc::new(push_loc)),
            },
        });

        // Request GCD trigger (Knockback triggers Attack GCD)
        gcd_writer.write(TriggerGcd {
            ent: *ent,
            typ: GcdType::Attack,
        });
    }
}
