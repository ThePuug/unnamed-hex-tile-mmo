use bevy::prelude::*;
use std::time::Duration;
use crate::common::{
    components::{entity_type::*, heading::*, reaction_queue::*, resources::*, gcd::Gcd, ActorAttributes, Loc},
    message::{AbilityType, Event as GameEvent, Try},
    plugins::nntree::NNTree,
    systems::targeting::*,
};

/// Client system to predict BasicAttack ability usage
/// Predicts GCD activation to prevent double-attacks
/// Does NOT predict threat insertion (server-authoritative)
pub fn predict_basic_attack(
    mut try_reader: EventReader<Try>,
    player_query: Query<(&Loc, &Heading), With<crate::common::components::Actor>>,
    mut player_gcd_query: Query<Option<&mut Gcd>, With<crate::common::components::Actor>>,
    entity_query: Query<(&EntityType, &Loc)>,
    nntree: Res<NNTree>,
    time: Res<Time>,
) {
    for event in try_reader.read() {
        if let GameEvent::UseAbility { ent, ability: AbilityType::BasicAttack } = event.event {
            // Input system already checked GCD - if we got this Try event, it's valid
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

            if let Some(_target_ent) = target_opt {
                // DON'T predict threat insertion into remote queues
                // Server will send InsertThreat event after validation
                // This avoids non-deterministic deduplication issues with network latency

                // DO predict GCD activation (prevents double-press before server confirmation)
                if let Ok(Some(mut gcd)) = player_gcd_query.get_mut(ent) {
                    gcd.activate(
                        crate::common::systems::combat::gcd::GcdType::Attack,
                        std::time::Duration::from_secs(1),
                        time.elapsed(),
                    );
                }
            }
        }
    }
}

/// Client system to predict Dodge ability usage
/// Optimistically clears queue and consumes stamina before server confirmation
pub fn predict_dodge(
    mut try_reader: EventReader<Try>,
    mut query: Query<(&mut ReactionQueue, &mut Stamina, &ActorAttributes, Option<&mut Gcd>)>,
    time: Res<Time>,
) {
    for event in try_reader.read() {
        if let GameEvent::UseAbility { ent, ability: AbilityType::Dodge } = event.event {
            if let Ok((mut queue, mut stamina, _attrs, gcd_opt)) = query.get_mut(ent) {
                // Input system already checked GCD - if we got this Try event, it's valid
                // Fixed dodge cost
                let dodge_cost = 60.0;

                // Check if we have enough stamina
                if stamina.state >= dodge_cost && !queue.is_empty() {
                    // Optimistically clear queue
                    let _cleared_count = queue.threats.len();
                    queue.threats.clear();

                    // Consume stamina
                    stamina.state -= dodge_cost;
                    stamina.step = stamina.state;

                    // Predict GCD activation (prevents double-press before server confirmation)
                    if let Some(mut gcd) = gcd_opt {
                        gcd.activate(
                            crate::common::systems::combat::gcd::GcdType::Attack,
                            std::time::Duration::from_secs(1),
                            time.elapsed(),
                        );
                    }
                }
            }
        }
    }
}

// REMOVED: Client-side threat expiration prediction
// This was causing player frustration due to desync between predicted and actual damage
// Server now handles all threat resolution authoritatively
