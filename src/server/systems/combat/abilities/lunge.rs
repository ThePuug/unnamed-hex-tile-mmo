use bevy::prelude::*;
use crate::{
    common::{
        components::{entity_type::*, heading::*, resources::*, Loc, reaction_queue::DamageType, gcd::Gcd},
        message::{AbilityFailReason, AbilityType, Do, Try, Event as GameEvent},
        plugins::nntree::*,
        systems::{targeting::*, combat::gcd::GcdType},
    },
};

/// Handle Lunge ability (Q key)
/// - 20 stamina cost
/// - 40 base damage
/// - 4 hex range
/// - Teleports caster adjacent to target
/// - Triggers Attack GCD
pub fn handle_lunge(
    mut commands: Commands,
    mut reader: EventReader<Try>,
    entity_query: Query<(&EntityType, &Loc, Option<&crate::common::components::behaviour::PlayerControlled>)>,
    loc_heading_query: Query<(&Loc, &Heading)>,
    mut stamina_query: Query<&mut Stamina>,
    mut gcd_query: Query<&mut Gcd>,
    respawn_query: Query<&RespawnTimer>,
    nntree: Res<NNTree>,
    mut writer: EventWriter<Do>,
    time: Res<Time>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target_loc: _ } } = event else {
            continue;
        };

        // Filter for Lunge only
        let Some(AbilityType::Lunge) = (ability == &AbilityType::Lunge).then_some(ability) else {
            continue;
        };

        // Check if caster is dead (has RespawnTimer)
        if respawn_query.get(*ent).is_ok() {
            // Dead players can't use abilities - silently ignore
            continue;
        }

        // Check GCD (must not be on cooldown)
        let Ok(gcd) = gcd_query.get(*ent) else {
            continue;
        };

        if gcd.is_active(time.elapsed()) {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OnCooldown,
                },
            });
            continue;
        }

        // Get caster's location and heading
        let Ok((caster_loc, caster_heading)) = loc_heading_query.get(*ent) else {
            continue;
        };

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
                entity_query.get(target_ent).ok().map(|(et, _, _)| *et)
            },
            |target_ent| {
                // Check if entity is player-controlled
                entity_query.get(target_ent).ok().and_then(|(_, _, pc_opt)| pc_opt).is_some()
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

        // Check range (must be within 4 hexes for Lunge)
        let distance = caster_loc.flat_distance(&target_loc) as u32;

        if distance > 4 || distance < 1 {
            // Target is out of range (or we're already on top of them)
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::OutOfRange,
                },
            });
            continue;
        }

        // Check stamina (20 cost)
        let lunge_stamina_cost = 20.0;
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        if stamina.state < lunge_stamina_cost {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::InsufficientStamina,
                },
            });
            continue;
        }

        // Consume stamina
        stamina.state -= lunge_stamina_cost;
        stamina.step = stamina.state;

        // Broadcast updated stamina
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: crate::common::message::Component::Stamina(*stamina),
            },
        });

        // Find landing position: adjacent to target, closest to caster
        let target_neighbors = (*target_loc).neighbors();
        let landing_loc = target_neighbors
            .iter()
            .min_by_key(|neighbor_loc| caster_loc.flat_distance(neighbor_loc))
            .copied()
            .unwrap_or(*target_loc); // Fallback to target loc if no neighbors

        // Update caster's location (teleport)
        commands.entity(*ent).insert(Loc::new(landing_loc));

        // Broadcast location update to clients
        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: crate::common::message::Component::Loc(Loc::new(landing_loc)),
            },
        });

        // Deal damage (40 base damage)
        commands.trigger_targets(
            Try {
                event: GameEvent::DealDamage {
                    source: *ent,
                    target: target_ent,
                    base_damage: 40.0,
                    damage_type: DamageType::Physical,
                },
            },
            target_ent,
        );

        // Trigger Attack GCD immediately (prevents race conditions)
        if let Ok(mut gcd) = gcd_query.get_mut(*ent) {
            let gcd_duration = std::time::Duration::from_secs(1); // 1s for Attack GCD (ADR-006)
            gcd.activate(GcdType::Attack, gcd_duration, time.elapsed());
        }
    }
}
