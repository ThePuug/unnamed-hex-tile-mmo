use bevy::prelude::*;
use std::time::Duration;
use crate::common::{
    components::{entity_type::*, heading::*, reaction_queue::*, resources::*, ActorAttributes, Loc},
    message::{AbilityType, Event as GameEvent, Try},
    plugins::nntree::NNTree,
    systems::{combat::queue as queue_utils, targeting::*},
};

/// Client system to predict BasicAttack ability usage
/// Optimistically inserts threat into target's queue before server confirmation
pub fn predict_basic_attack(
    mut try_reader: EventReader<Try>,
    player_query: Query<(&Loc, &Heading), With<crate::common::components::Actor>>,
    mut target_query: Query<(Option<&mut ReactionQueue>, Option<&ActorAttributes>)>,
    entity_query: Query<(&EntityType, &Loc)>,
    nntree: Res<NNTree>,
    server: Res<crate::client::resources::Server>,
    time: Res<Time>,
) {
    for event in try_reader.read() {
        if let GameEvent::UseAbility { ent, ability: AbilityType::BasicAttack } = event.event {
            // Get player's location and heading
            let Ok((player_loc, player_heading)) = player_query.get(ent) else { continue; };

            // Find target using same logic as server
            let target_opt = select_target(
                ent,
                *player_loc,
                *player_heading,
                None, // No tier lock in MVP
                &nntree,
                |target_ent| entity_query.get(target_ent).ok().map(|(et, _)| *et),
            );

            if let Some(target_ent) = target_opt {
                // Predict threat insertion (immediate UI feedback)
                if let Ok((Some(mut queue), Some(attrs))) = target_query.get_mut(target_ent) {
                    let now_ms = server.current_time(time.elapsed().as_millis());
                    let now = Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);
                    let timer_duration = queue_utils::calculate_timer_duration(attrs);

                    let predicted_threat = QueuedThreat {
                        source: ent,
                        damage: 20.0,
                        damage_type: DamageType::Physical,
                        inserted_at: now,
                        timer_duration,
                    };

                    // Insert predicted threat (client sees it immediately)
                    queue_utils::insert_threat(&mut queue, predicted_threat, now);
                }
            }
        }
    }
}

/// Client system to predict Dodge ability usage
/// Optimistically clears queue and consumes stamina before server confirmation
pub fn predict_dodge(
    mut try_reader: EventReader<Try>,
    mut query: Query<(&mut ReactionQueue, &mut Stamina, &ActorAttributes)>,
) {
    for event in try_reader.read() {
        if let GameEvent::UseAbility { ent, ability: AbilityType::Dodge } = event.event {
            if let Ok((mut queue, mut stamina, _attrs)) = query.get_mut(ent) {
                // Calculate dodge cost (15% of max stamina as per ADR)
                let dodge_cost = stamina.max * 0.15;

                // Check if we have enough stamina
                if stamina.state >= dodge_cost && !queue.is_empty() {
                    // Optimistically clear queue
                    let _cleared_count = queue.threats.len();
                    queue.threats.clear();

                    // Consume stamina
                    stamina.state -= dodge_cost;
                    stamina.step = stamina.state;
                }
            }
        }
    }
}

/// Client system to predict local player damage when threats expire
/// Provides instant visual feedback for health changes
pub fn predict_threat_resolution(
    mut query: Query<(&mut ReactionQueue, &mut Health, &ActorAttributes), With<crate::common::components::Actor>>,
    time: Res<Time>,
    server: Res<crate::client::resources::Server>,
) {
    for (mut queue, mut health, attrs) in &mut query {
        // Calculate current time (using server time for consistency)
        let now_ms = server.current_time(time.elapsed().as_millis());
        let now = std::time::Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

        // Check for expired threats (same logic as server)
        let expired_threats = crate::common::systems::combat::queue::check_expired_threats(&queue, now);

        // Predict damage for each expired threat
        for threat in expired_threats {
            // Calculate final damage using Phase 2 mitigation
            let final_damage = crate::common::systems::combat::damage::apply_passive_modifiers(
                threat.damage,
                attrs,
                threat.damage_type,
            );

            // Apply predicted damage to health.step (not state - that's server-authoritative)
            health.step = (health.step - final_damage).max(0.0);

            info!("CLIENT PREDICTION: Threat expired, predicted damage: {:.1}, new health.step: {:.1}",
                final_damage, health.step);
        }

        // Remove expired threats from queue (client-side cleanup)
        // Server will send ApplyDamage events which will confirm/correct our prediction
        queue.threats.retain(|threat| {
            let time_since_insert = now.saturating_sub(threat.inserted_at);
            time_since_insert < threat.timer_duration
        });
    }
}
