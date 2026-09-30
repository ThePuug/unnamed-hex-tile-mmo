use bevy::prelude::*;
use std::time::Duration;
use common_bevy::{
    components::{entity_type::*, resources::*, status::Status, Loc, reaction_queue::{ReactionQueue, QueuedThreat}, recovery::{GlobalRecovery, get_ability_recovery_duration}},
    message::{AbilityFailReason, AbilityType, ClearType, Do, Try, Event as GameEvent},
    resources::map::Map,
    systems::combat::synergies::{apply_synergies, is_early, lockout, may_use, reacts_through, settle_combo},
};
use crate::resources::RunTime;

/// Seconds a Kick holds what it drives back: long enough that it does not
/// walk straight in again.
const STAGGER_SECS: f32 = 0.5;

/// Handle Kick ability — REACTIVE KICK
/// - `Tuning::kick_cost` stamina
/// - Clears all visible window threats
/// - Deals 75% Precision damage to adjacent threat sources
/// - Drives adjacent sources 4 tiles away over the ground (`leap::away`),
///   further by the kicker's hold (`ActorAttributes::hold`), and staggers them
/// - Synergy: Kick → Lunge
pub fn handle_kick(
    mut commands: Commands,
    mut reader: MessageReader<Try>,
    entity_query: Query<(&EntityType, &Loc)>,
    mut queue_query: Query<(&Loc, &mut ReactionQueue)>,
    mut stamina_query: Query<&mut Stamina>,
    attrs_query: Query<&common_bevy::components::ActorAttributes>,
    recovery_query: Query<&GlobalRecovery>,
    synergy_query: Query<&common_bevy::components::recovery::SynergyUnlock>,
    combo_query: Query<&common_bevy::components::recovery::Combo>,
    respawn_query: Query<&RespawnTimer>,
    time: Res<Time>,
    runtime: Res<RunTime>,
    map: Res<Map>,
    mut statuses: Query<&mut Status>,
    mut writer: MessageWriter<Do>,
) {
    for event in reader.read() {
        let Try { event: GameEvent::UseAbility { ent, ability, target: _ } } = event else {
            continue;
        };

        if *ability != AbilityType::Kick {
            continue;
        }

        // Check if caster is dead
        if respawn_query.get(*ent).is_ok() {
            continue;
        }

        // Out of lockout, or taking the follow-up the last ability offered
        // Out of lockout, the follow-up the last ability offered, or a
        // reaction Preparation lets through the lockout
        let recovery = recovery_query.get(*ent).ok();
        if !may_use(AbilityType::Kick, recovery, synergy_query.get(*ent).ok(), combo_query.get(*ent).ok())
            && !reacts_through(AbilityType::Kick, recovery, attrs_query.get(*ent).ok())
        {
            writer.write(Do { event: GameEvent::AbilityFailed { ent: *ent, reason: AbilityFailReason::OnCooldown } });
            continue;
        }

        // Get caster's attributes and location
        let Ok(caster_attrs) = attrs_query.get(*ent) else {
            continue;
        };

        let caster_loc = {
            let Ok((loc, _)) = queue_query.get(*ent) else {
                writer.write(Do {
                    event: GameEvent::AbilityFailed {
                        ent: *ent,
                        reason: AbilityFailReason::NoTargets,
                    },
                });
                continue;
            };
            *loc
        };

        // Get visible window threats (collect to drop borrow)
        let visible_threats: Vec<QueuedThreat> = {
            let Ok((_, queue)) = queue_query.get(*ent) else {
                continue;
            };
            queue.threats.iter()
                .take(queue.window_size)
                .copied()
                .collect()
        };

        if visible_threats.is_empty() {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::NoTargets,
                },
            });
            continue;
        }

        let kick_stamina_cost = common_bevy::tuning::tuning().kick_cost;
        let Ok(mut stamina) = stamina_query.get_mut(*ent) else {
            continue;
        };

        if stamina.state < kick_stamina_cost {
            writer.write(Do {
                event: GameEvent::AbilityFailed {
                    ent: *ent,
                    reason: AbilityFailReason::InsufficientStamina,
                },
            });
            continue;
        }

        // Consume stamina
        stamina.state -= kick_stamina_cost;
        stamina.step = stamina.state;

        writer.write(Do {
            event: GameEvent::Incremental {
                ent: *ent,
                component: common_bevy::message::Component::Stamina(*stamina),
            },
        });

        let now_ms = time.elapsed().as_millis() + runtime.elapsed_offset;
        let now = Duration::from_millis(now_ms.min(u64::MAX as u128) as u64);

        use common_bevy::systems::combat::queue::create_threat;

        // Process each visible threat: deal damage and knockback adjacent sources
        for threat in &visible_threats {
            // Only affect sources that are alive and adjacent
            if respawn_query.get(threat.source).is_ok() {
                continue;
            }
            let Ok((_, source_loc)) = entity_query.get(threat.source) else {
                continue;
            };
            if caster_loc.flat_distance(source_loc) != 1 {
                continue;
            }

            // Deal 75% Precision damage via threat insertion
            let kick_damage = caster_attrs.precision() * 0.75;

            if let Ok((_, mut target_queue)) = queue_query.get_mut(threat.source) {
                if let Ok(target_attrs) = attrs_query.get(threat.source) {
                    let kick_threat = create_threat(
                        *ent,
                        target_attrs,
                        caster_attrs,
                        kick_damage,
                        Some(AbilityType::Kick),
                        now,
                        0.0,
                    );

                    common_bevy::systems::combat::queue::insert_threat(&mut target_queue, kick_threat, now);

                    writer.write(Do {
                        event: GameEvent::InsertThreat {
                            ent: threat.source,
                            threat: kick_threat,
                        },
                    });
                }
            }

            // Driven back over the ground, away from the kicker, and held
            // there a moment; with nowhere to go it is only held
            let tiles = (4.0 * caster_attrs.hold()).round() as usize;
            if let Some(landing) = crate::systems::combat::leap::away(&map, **source_loc, *caster_loc, tiles) {
                let pushed = landing.flat_distance(&**source_loc) as u16;
                crate::systems::combat::leap::slide(threat.source, landing, pushed * 125, None, &mut commands, &mut writer);
            }
            crate::systems::combat::landing::update(threat.source, &mut statuses, &mut commands, &mut writer, |status| status.hold(STAGGER_SECS));
        }

        // Drain visible threats from caster's queue
        if let Ok((_, mut caster_queue)) = queue_query.get_mut(*ent) {
            let count = visible_threats.len();
            common_bevy::systems::combat::queue::clear_threats(&mut caster_queue, ClearType::First(count));

            writer.write(Do {
                event: GameEvent::ClearQueue {
                    ent: *ent,
                    clear_type: ClearType::First(count),
                },
            });
        }

        // Broadcast ability success
        writer.write(Do {
            event: GameEvent::UseAbility {
                ent: *ent,
                ability: AbilityType::Kick,
                target: visible_threats.first().map(|threat| threat.source),
            },
        });

        // Trigger recovery lockout
        let (prior, offer) = (recovery_query.get(*ent).ok().copied(), synergy_query.get(*ent).ok().copied());
        let early = is_early(AbilityType::Kick, prior.as_ref(), offer.as_ref());
        let recovery = lockout(AbilityType::Kick, prior.as_ref(), offer.as_ref(), visible_threats.first().and_then(|threat| attrs_query.get(threat.source).ok()));
        commands.entity(*ent).insert(recovery);

        // Apply synergies (Kick → Lunge)
        let Ok(attrs) = attrs_query.get(*ent) else {
            continue;
        };
        apply_synergies(*ent, AbilityType::Kick, &recovery, attrs, attrs, &mut commands);
        settle_combo(*ent, AbilityType::Kick, early, get_ability_recovery_duration(AbilityType::Kick), attrs, combo_query.get(*ent).ok(), &mut commands);
    }
}
